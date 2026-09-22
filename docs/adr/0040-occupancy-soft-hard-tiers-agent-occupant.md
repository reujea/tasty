# ADR-0040: 점유를 약한/강한(soft/hard) 2계층으로 나누고 AI 에이전트를 점유 주체로 일반화한다

- **Status**: Superseded by [ADR-0052](0052-attach-heartbeat-ttl-hard-occupancy-release.md) (부분) — "점유 해제·수명" 절의 강한 점유 해제 사유가 EOF 단독에서 "연결 생존 판정 실패(EOF 또는 attach heartbeat TTL 만료)"로 좁게 확장됨. 그 외(soft 점유 정책, 시각 표현 등)는 그대로 유효.
- **Date**: 2026-07-07
- **Tags**: occupation, soft-occupy, hard-occupy, actors, ai-agent, child-terminal, attach, readonly, marker, focus-independence, adr-0007, adr-0032
- **Group**: remote-attach

## Context

지금 "점유(occupation)" 는 **원격 attach 전용·단일 계층** 이다 ([`concepts/actors.md`](../concepts/actors.md), [`dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md)):

- 원격 접속 사용자만 surface/workspace 를 **배타 claim** 할 수 있다(`AttachRegistry.surface_locks`).
- 점유되면 로컬 사용자·AI 에이전트는 그 대상에 대해 **readonly**(서버측 placeholder 렌더 + `apply_send_to_surface` 로컬 입력 거부), 서버측 readonly 뷰는 **3초 polling**(`AttachPoll`) 으로 갱신된다.
- **로컬 사용자만** force-detach 로 끊을 수 있다.

반면 AI 에이전트는 actors 표에 **"점유 불필요/없음"** 으로 못박혀 있고, surface 조작은 `surface.send`(주입) / `surface.read`(읽기) 의 **fire-and-forget** 로만 한다. 여기에 두 공백이 있다:

1. **가시적 계약의 부재.** 에이전트가 어떤 surface(터미널 등)를 지속적으로 조종해도, 그것을 보는 로컬 사용자에게 *"이건 지금 다른 주체가 조종 중이며, 언제든 닫히거나 상태가 바뀔 수 있다"* 는 사실이 **아무 데도 표시되지 않는다.** send/read 는 흔적을 남기지 않으므로 사용자는 자기 입력과 에이전트 주입이 뒤섞이는 것을 인지할 수 없다.
2. **강한 통제가 필요한 경우의 부재.** 에이전트가 어떤 터미널을 배타적으로 구동하며 사용자 입력 혼입을 원천 차단하고 mirror 로만 관찰시키고 싶어도(예: 장시간 명령을 원자적으로 돌리는 원격 세션), 에이전트에겐 그럴 관문이 없다 — 그 메커니즘(readonly + polling + lock)은 이미 attach 에 존재하는데 원격 사용자만 쓸 수 있다.

즉 **점유의 메커니즘은 이미 있는데(=강한 점유), 주체가 원격 사용자로 한정** 되어 있고, **그보다 약한 "표시만 하는" 단계가 없다.**

## Decision

점유를 **두 계층** 으로 나누고, 점유 주체를 원격 사용자에 더해 **AI 에이전트까지 일반화** 한다. 두 계층은 **시각적으로 반드시 구분** 되며, 어느 계층이든 점유는 **점유 주체 본인(self-release) 또는 로컬 사용자(force-detach) 만** 끊을 수 있다 — 그 외 주체는 남의 점유를 끊지 못한다(기존 주권 원칙 보존, attach 의 `release`/`force_detach` 2경로와 동일).

### 약한 점유 (soft occupy)

- **표시만 한다.** "이 surface 는 어떤 주체에게 점유당하는 중" 이라는 advisory 마커를 붙인다. **write 제한 없음** — 로컬 사용자는 평소처럼 자유롭게 입력·조작한다.
- 이 마커의 의미(=`surface.send`/`surface.read` 와의 결정적 차이)는 **"이 터미널/surface 는 언제든 점유 주체에 의해 닫히거나 상태가 바뀔 수 있다"는 사실을 명시적으로 고지** 하는 것이다. fire-and-forget 주입엔 없던 *지속적·가시적 관계* 를 만든다.
- 사용자가 마커를 무시하고 그 surface 를 그대로 써도 무방하다. soft 점유는 **협조적 신호이지 강제가 아니다.**
- **입력 독립성 best-effort (방향성).** soft 점유는 사용자 write 가 살아있으므로, 점유 주체가 명령을 보낼 때 **커맨드라인에 사용자가 이미 쳐둔 잔여 입력(예: 반쯤 친 `asdfkljbasfdklj`)과 무관하게 점유 주체가 의도한 동작(예: `ls | grep …`)이 실행되도록 "최대한" 보장** 하는 것을 지향한다. 단순 `surface.send` 가 현재 라인 버퍼에 그대로 append 하여 `asdfkljbasfdkljls | grep …` 로 오염되는 것과 구분되는 지점이다. 다만 잔여 입력을 안전하게 비우는(라인 클리어 등) 방법은 컨텍스트마다 다르고 — 셸 프롬프트 / TUI·에디터 / 패스워드 프롬프트 / 포그라운드 실행 중 — **불가능하거나 부적절한 상황이 많다.** 따라서 이것은 단일 메커니즘이 아니라 **상황별로 다르게 구현되는 방향성** 이며, 본 ADR 은 *목표(=점유 주체 행동의 잔여-입력 비오염)* 만 명시하고 구체 처리(모드 감지·라인 클리어 시퀀스 등)는 구현 층위에 위임한다. 이 best-effort 라인 정리 자체가 사용자의 미제출 입력을 건드리는 상태 변화이지만, soft 점유의 **가시적 마커가 "언제든 상태가 바뀔 수 있음" 을 이미 고지** 하므로 계약 안에 있다.

### 강한 점유 (hard occupy)

- 마커를 붙이되 **약한 점유와 다른 표시** 를 쓴다(두 상태의 구분이 필수 요건).
- 동시에 **기존 attach 점유와 동일한 메커니즘** 을 적용한다: 로컬 사용자에게 **readonly**(입력 차단, mirror 관찰), **polling 갱신** 상태.
- 단 로컬 사용자는 여전히 그 surface 를 **닫거나 점유를 끊을 수 있다** (force-detach). 강한 점유는 사용자의 관찰·종료 권한을 뺏지 않는다.

### 주체·범위

- **점유 주체 = 원격 사용자 | AI 에이전트.** 기존 원격 attach 점유는 본 모델의 **강한 점유의 한 사례** 로 흡수된다(이름만 "강한 점유" 로 승격, 메커니즘 불변).
- **점유는 surface 의 생성 방식과 무관하게 걸린다.** 새로 만든 surface 든, **이미 살아있는(로컬 사용자가 쓰던 것 포함) surface** 든, 어떤 기능이 점유를 걸 수 있다. 이는 기존 attach 가 *이미 살아있는* 원격 surface 를 점유하는 것과 동형이다 — "spawn 한 것만 점유" 같은 제한은 점유 개념 자체엔 없다.
- **본 ADR 은 점유 개념 그 자체만 정의하며, 특정 소비자 기능과 독립적이다.** 예컨대 `terminal`/`claude`/`codex` 명령이 "자기가 spawn 한 child surface 만 점유" 하는 것은 그 명령들이 점유를 소비하는 **한 사례의 정책** 일 뿐, 점유가 걸릴 수 있는 대상 범위를 규정하지 않는다. 점유 계획과 child-terminal spawn 구현은 별개 축이다.
- 대상당 점유 주체는 **1:1**(배타), 주체당 대상은 **1:N** — 기존 다중성 규칙 유지.
- **현 시점 soft 점유 소비자 = `terminal` 명령 + claude/codex 플러그인 뿐.** 살아있는 임의 surface 에 soft 점유를 거는 **직접 명령(CLI/IPC)은 두지 않는다** — 필요해지면 후속으로 추가한다. hard 점유는 기존 attach 경로 그대로다.

### 시각 표현 (터미널 테두리)

점유·완료 상태는 **터미널 테두리 색** 하나의 채널로 표시하며, 색으로 구분한다. **본 ADR 은 이 채널 구조(3상태를 색 하나로 구분) 와 우선순위(아래)라는 큰 틀만 규정한다 — 구체적인 색값·토큰은 디자인이 결정한다(디자인이 SoT).** 색은 전부 Theme semantic token 에서 온다(raw px/`from_rgb` 금지, [ADR-0033](0033-ui-color-semantic-role-only.md)).

디자인 확정값(2026-07-07, 디자인 정본 소유 — 세부는 디자인이 갱신할 수 있으며 소스는 그것을 따른다):

- **약한 점유(soft)** = green (`--tasty-accent-occupied-soft`)
- **강한 점유(hard)** = peach (`--tasty-accent-occupied-hard`) — 기존 원격 attach 테두리(`accent-attention`) 색을 점유 역할로 승격
- **완료 알림** = blue (`accent-primary`) — 기존 surface-highlight 테두리 그대로, 포커스 시 사라짐([ADR-0039](0039-surface-highlight-shared-primitive.md)). 라이브 소스가 이미 쓰는 색을 유지한다.

**테두리 채널 우선순위**: 점유와 완료 알림은 **같은 테두리 채널을 공유** 하므로 동시에 못 그린다. 점유 중이면 **점유 테두리가 완료 알림 테두리를 덮는다** — 점유된 surface 엔 완료 알림용 테두리를 따로 표시하지 않는다. 단 완료 알림의 **다른 채널(탭 제목·워크스페이스 배지)은 surface-highlight 그대로 동작** 한다 — 점유가 덮는 것은 테두리 채널뿐이다.

### 점유 해제·수명

- **능동 해제는 명시적 해제뿐** (위 Decision: self-release | force-detach). 시간 만료·유휴 자동 해제는 없다.
- **강한 점유**: 기존 attach 수명 그대로(연결 EOF / force-detach).
- **약한 점유**: 점유 주체가 죽어도 **즉시 자동 해제되지 않는다**(soft 는 연결 기반이 아닐 수 있어 죽음을 항상 인지할 수 없음). 대신:
  - 죽음을 **인지 가능한 경우(명시적 해제)** 는 그때 풀린다.
  - 그 외에는 soft 점유가 내부적으로 **parent(점유 주체에 대응하는 surface)를 기록만** 해둔다.
  - 그 surface 가 **focus 될 때**, 기록된 parent surface 가 더 이상 존재하지 않으면 → 그 시점에 **점유 없음으로 지연 갱신(청소)** 한다. (surface-highlight 의 "실 포커스 시 해제"([ADR-0039](0039-surface-highlight-shared-primitive.md))와 동일하게 *실 사용자 포커스* 기준이라 원칙1 안전.)

## Consequences

- **얻은 것**:
  - 에이전트가 구동하는 터미널이 **로컬 사용자에게 가시적** 이 된다 — "누가 조종 중이고, 언제든 닫힐 수 있다" 를 마커로 고지. 사용자 혼란(내 입력 vs 에이전트 주입) 제거.
  - 에이전트가 **원자적·배타적 구동** 이 필요할 때 강한 점유로 관문을 통과할 수 있다(child-terminal 를 살아있는 대화형 세션으로 잡고 readonly mirror 로 사용자에게 노출). attach 의 검증된 메커니즘 재사용.
  - 점유 개념이 원격 attach 에 갇히지 않고 **주체 중립 모델** 로 승격 → actors 모델의 일관성 강화.
- **잃은 것**:
  - actors 모델의 "AI Agent = 점유 없음" 이라는 단정이 사라진다 — [`concepts/actors.md`](../concepts/actors.md) · [`identity.md`](../identity.md) §2.1 표/서술 개정 필요.
  - soft/hard 테두리 색은 **신규 시각 요소** 였고, 디자인이 소유 토큰을 확정했다(2026-07-07: soft=green / hard=peach, 완료=blue 유지 — semantic 토큰 + gallery specimen 포함).
- **운영 비용 / 유지 부담**:
  - 강한 점유(attach `AttachRegistry`)와 약한 점유를 **하나의 통합 점유 레지스트리** 로 묶는 방향을 선호하되, 실현가능성·수정범위는 구현 단계에서 확정한다(통합이 적절하면 통합, 아니면 분리 유지).
  - 약한 점유의 지연 해제(focus 시 parent 부재 청소)는 포커스 경로에 점유 정합 로직을 얹는다 — surface-highlight 의 실-포커스-해제 경로와 같은 자리라 재사용 여지가 있다.

## Alternatives Considered

- **A: 단일 계층 유지, 에이전트에도 기존(강한) 점유만 개방** — soft 가 없으면 "표시만 하고 사용자는 자유" 라는 가장 흔한(그리고 원칙1 마찰이 가장 적은) 사용례를 표현할 수 없다. 에이전트가 터미널을 구동한다는 사실을 알리는 데 매번 사용자 입력을 차단하는 건 과하다. 기각.
- **B: 점유 없이 send/read 만 유지(현행)** — 지속적·가시적 계약이 없어 사용자가 "이 터미널이 조종당하는 중이며 언제든 닫힐 수 있음" 을 알 수 없다. child-terminal 개념의 핵심 요구를 못 채운다. 기각.
- **C: 에이전트 점유는 항상 강한 점유(readonly 강제)** — 사용자 자유를 불필요하게 뺏는다. 대부분의 에이전트 구동은 관찰·협조로 충분하다. 기각(→ soft 기본, hard 선택).
- **D: 마커를 soft/hard 공용 단일 표시로** — 두 상태의 사용자 대처가 다르다(자유 입력 가능 vs readonly). 구분 없으면 사용자가 입력이 먹히는지 예측할 수 없다. 기각(구분 필수).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 에이전트가 **로컬 사용자가 활발히 포커스·입력 중인 surface 를 강한 점유(readonly 강제)** 로 잡는 정책이 필요할 때 — 점유가 걸릴 수 있는 대상엔 제한이 없지만(위), *언제 그렇게 하는 게 원칙1 상 적절한가* 는 소비자별 정책 판단이 남는다. 남용 방지 규칙이 요구되면 재검토한다.
- soft/hard 외에 **제3의 계층**(예: "제안만 하고 write 는 확인 후" 반-강제) 요구가 생길 때.
- 강한 점유의 lease 수명·자동 해제 정책이 attach 의 EOF 기반 해제와 **다른 규칙** 을 요구할 때.
- 한 대상에 **다중 주체 동시 점유**(현재 1:1 배타) 요구가 생길 때.

## References

- 주체·기존 점유 모델: [`concepts/actors.md`](../concepts/actors.md)
- 정체성 원칙(사용자 행동 ↔ 에이전트 행동 분리): [`identity.md`](../identity.md) §2.1
- 강한 점유의 기존 메커니즘(mirror/lock/readonly/polling/force-detach): [`dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md)
- attach 대상=원격, 로컬 self-attach debug 격리: [`ADR-0007`](0007-attach-targets-remote.md)
- 원격 프로필 2-레이어: [`ADR-0032`](0032-remote-attach-two-layer-split.md)
